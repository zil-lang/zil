//! Independent bounded scalar model. No compiler module supplies expected values.
use crate::{protocol, search};
use serde_json::{Value, json};

pub const CONTRACT: &str = "zil-scalar-native/1";
pub const PROPERTY: &str = "scalar.native-equals-model";
pub const MAX_PROGRAMS: usize = 32;
const OPS: [&str; 8] = ["+", "-", "*", "/", "%", "+%", "-%", "*%"];

#[derive(Clone)]
enum Expr {
    Lit(i128),
    Neg(Box<Expr>),
    Bin(String, Box<Expr>, Box<Expr>),
}
fn bounds(width: u32) -> (i128, i128) {
    let top = 1i128 << (width - 1);
    (-top, top - 1)
}
impl Expr {
    fn parse(v: &Value, width: u32, depth: usize, nodes: &mut usize) -> Result<Self, String> {
        *nodes += 1;
        if *nodes > 256 || depth > 8 {
            return Err("scalar AST node/depth quota exceeded".into());
        }
        match v["kind"].as_str() {
            Some("lit") => {
                let text = v["value"]
                    .as_str()
                    .ok_or("literal must be a decimal string")?;
                let n: i128 = text.parse().map_err(|_| "invalid scalar literal")?;
                if (text != n.to_string() && text != "-0")
                    || n < bounds(width).0
                    || n > bounds(width).1
                {
                    return Err("noncanonical or out-of-range scalar literal".into());
                }
                Ok(Self::Lit(n))
            }
            Some("neg") => Ok(Self::Neg(Box::new(Self::parse(
                &v["value"],
                width,
                depth + 1,
                nodes,
            )?))),
            Some("bin") if OPS.contains(&v["op"].as_str().unwrap_or("")) => Ok(Self::Bin(
                v["op"].as_str().unwrap().into(),
                Box::new(Self::parse(&v["left"], width, depth + 1, nodes)?),
                Box::new(Self::parse(&v["right"], width, depth + 1, nodes)?),
            )),
            _ => Err("invalid scalar expression".into()),
        }
    }
    fn json(&self) -> Value {
        match self {
            Self::Lit(n) => lit(*n),
            Self::Neg(v) => json!({"kind":"neg","value":v.json()}),
            Self::Bin(op, a, b) => bin(op, a.json(), b.json()),
        }
    }
    fn eval(&self, width: u32) -> Result<i128, ()> {
        match self {
            Self::Lit(n) => Ok(*n),
            Self::Neg(a) => operation("-", 0, a.eval(width)?, width),
            Self::Bin(op, a, b) => operation(op, a.eval(width)?, b.eval(width)?, width),
        }
    }
    fn emit(&self, mutation: bool) -> String {
        match self {
            Self::Lit(n) if *n < 0 => format!("({n})"),
            Self::Lit(n) => n.to_string(),
            Self::Neg(a) => format!("(-{})", a.emit(mutation)),
            Self::Bin(op, a, b) => format!(
                "({} {} {})",
                a.emit(mutation),
                if mutation && op == "-" { "+" } else { op },
                b.emit(mutation)
            ),
        }
    }
    fn cost(&self) -> (usize, u128) {
        match self {
            Self::Lit(n) => (1, n.unsigned_abs()),
            Self::Neg(v) => {
                let c = v.cost();
                (c.0 + 1, c.1)
            }
            Self::Bin(_, a, b) => {
                let a = a.cost();
                let b = b.cost();
                (1 + a.0 + b.0, a.1 + b.1)
            }
        }
    }
    fn smaller(&self) -> Vec<Self> {
        let mut out = vec![Self::Lit(0), Self::Lit(1), Self::Lit(-1)];
        match self {
            Self::Lit(n) => out.push(Self::Lit(n / 2)),
            Self::Neg(a) => {
                out.push((**a).clone());
                out.extend(a.smaller().into_iter().map(|v| Self::Neg(Box::new(v))));
            }
            Self::Bin(op, a, b) => {
                out.extend([(**a).clone(), (**b).clone()]);
                out.extend(
                    a.smaller()
                        .into_iter()
                        .map(|v| Self::Bin(op.clone(), Box::new(v), b.clone())),
                );
                out.extend(
                    b.smaller()
                        .into_iter()
                        .map(|v| Self::Bin(op.clone(), a.clone(), Box::new(v))),
                );
            }
        }
        out.retain(|v| v.cost() < self.cost());
        out.truncate(128);
        out
    }
}

/// Every operand is already within i32/i64. Even a 64-bit product fits i128.
fn operation(op: &str, a: i128, b: i128, width: u32) -> Result<i128, ()> {
    let (min, max) = bounds(width);
    if (op == "/" || op == "%") && (b == 0 || (a == min && b == -1)) {
        return Err(());
    }
    let n = match op {
        "+" | "+%" => a + b,
        "-" | "-%" => a - b,
        "*" | "*%" => a * b,
        "/" => a / b,
        "%" => a % b,
        _ => unreachable!("validated scalar operator"),
    };
    if op.len() == 2 {
        Ok((n - min).rem_euclid(1i128 << width) + min)
    } else if n < min || n > max {
        Err(())
    } else {
        Ok(n)
    }
}

struct Program {
    width: u32,
    mode: String,
    expr: Expr,
    other: Option<Expr>,
    operator: String,
    count: usize,
    left: bool,
}
impl Program {
    fn parse(v: &Value) -> Result<Self, String> {
        let width = match protocol::integer(&v["width"]) {
            Some(32) => 32,
            Some(64) => 64,
            _ => return Err("scalar width must be 32 or 64".into()),
        };
        let mode = v["mode"].as_str().ok_or("missing scalar mode")?;
        if !["expr", "ct", "locals", "loop", "short", "branch"].contains(&mode) {
            return Err("unknown scalar mode".into());
        }
        let mut nodes = 0;
        let expr = Expr::parse(&v["expr"], width, 0, &mut nodes)?;
        let other = if ["locals", "loop", "branch"].contains(&mode) {
            Some(Expr::parse(&v["other"], width, 0, &mut nodes)?)
        } else {
            None
        };
        let op = v["operator"].as_str().unwrap_or("");
        if (mode == "locals" && !OPS.contains(&op))
            || (mode == "loop" && !["+", "-", "+%", "-%"].contains(&op))
            || (mode == "short" && !["&&", "||"].contains(&op))
        {
            return Err("invalid scalar mode operator".into());
        }
        let count = if mode == "loop" {
            protocol::integer(&v["count"])
                .filter(|n| (0..=12).contains(n))
                .ok_or("invalid bounded loop count")? as usize
        } else {
            0
        };
        let left = if mode == "short" {
            v["left"].as_bool().ok_or("short left must be bool")?
        } else {
            false
        };
        Ok(Self {
            width,
            mode: mode.into(),
            expr,
            other,
            operator: op.into(),
            count,
            left,
        })
    }
    fn eval(&self) -> Result<i128, ()> {
        let w = self.width;
        if self.mode == "short" {
            let right = || self.expr.eval(w).map(|n| n == 0);
            let v = if self.operator == "&&" {
                self.left && right()?
            } else {
                self.left || right()?
            };
            return Ok(i128::from(v));
        }
        let mut n = self.expr.eval(w)?;
        if self.mode == "branch" {
            return if n < 0 {
                self.other.as_ref().unwrap().eval(w)
            } else {
                Ok(7)
            };
        }
        if self.mode == "locals" {
            n = operation(&self.operator, n, self.other.as_ref().unwrap().eval(w)?, w)?;
        }
        if self.mode == "loop" {
            for _ in 0..self.count {
                n = operation(&self.operator, n, self.other.as_ref().unwrap().eval(w)?, w)?;
            }
        }
        Ok(n)
    }
}
pub fn validate(program: &Value) -> Result<(), String> {
    Program::parse(program).map(|_| ())
}
pub fn expected(program: &Value) -> Result<Value, String> {
    let p = Program::parse(program)?;
    Ok(match p.eval() {
        Ok(value) => json!({"kind":"value","value":value.to_string()}),
        Err(()) => json!({"kind":if p.mode == "ct" {"ct-reject"} else {"trap"}}),
    })
}
pub fn source(program: &Value, mutation: bool) -> Result<String, String> {
    let p = Program::parse(program)?;
    let ty = format!("i{}", p.width);
    let first = p.expr.emit(mutation);
    let other = || p.other.as_ref().unwrap().emit(mutation);
    let body = match p.mode.as_str() {
        "ct" => format!("return #({first})"),
        "locals" => format!(
            "let a: {ty} = {first}\nlet b: {ty} = {}\nreturn a {} b",
            other(),
            p.operator
        ),
        "loop" => format!(
            "var acc: {ty} = {first}\nvar index: i32 = 0\nwhile index < {} {{ acc = acc {} {}\nindex = index + 1 }}\nreturn acc",
            p.count,
            p.operator,
            other()
        ),
        "short" => format!(
            "if {} {} zero({first}) {{ return 1 }} else {{ return 0 }}",
            p.left, p.operator
        ),
        "branch" => format!(
            "if negative({first}) {{ return {} }} else {{ return 7 }}",
            other()
        ),
        _ => format!("return {first}"),
    };
    Ok(format!(
        "fn zero(x: {ty}) -> bool => x == 0\nfn negative(x: {ty}) -> bool => x < 0\nfn value() -> {ty} {{\n{body}\n}}\n"
    ))
}
pub fn lit(n: i128) -> Value {
    json!({"kind":"lit","value":n.to_string()})
}
pub fn bin(op: &str, a: Value, b: Value) -> Value {
    json!({"kind":"bin","op":op,"left":a,"right":b})
}
pub fn program(width: u32, mode: &str, expr: Value) -> Value {
    json!({"width":width,"mode":mode,"expr":expr})
}
pub fn trace(programs: Vec<Value>, seed: Option<i64>) -> Value {
    let mut v = json!({"format":"roam/1","contract":CONTRACT,"steps":programs.into_iter().map(|program| json!({"op":"run","program":program})).collect::<Vec<_>>()});
    if let Some(seed) = seed {
        v["seed"] = json!(seed);
    }
    v
}
struct Rng(u32);
impl Rng {
    fn next(&mut self, n: usize) -> usize {
        ((search::random_next(&mut self.0) as u64 * n as u64) >> 32) as usize
    }
    fn expr(&mut self, width: u32, depth: usize) -> Value {
        if depth == 3 || self.next(3) == 0 {
            return if self.next(8) == 0 {
                let b = bounds(width);
                lit(if self.next(2) == 0 { b.0 } else { b.1 })
            } else {
                lit(self.next(33) as i128 - 16)
            };
        }
        if self.next(6) == 0 {
            return json!({"kind":"neg","value":self.expr(width, depth+1)});
        }
        let op = OPS[self.next(OPS.len())];
        bin(op, self.expr(width, depth + 1), self.expr(width, depth + 1))
    }
}
pub fn generate(seed: i64, count: usize) -> Value {
    let mut rng = Rng(seed as u32);
    let modes = ["expr", "ct", "locals", "loop", "short", "branch"];
    let mut programs = Vec::new();
    for _ in 0..count.min(MAX_PROGRAMS) {
        let width = if rng.next(2) == 0 { 32 } else { 64 };
        let mode = modes[rng.next(modes.len())];
        let mut p = program(width, mode, rng.expr(width, 0));
        if ["locals", "loop", "branch"].contains(&mode) {
            p["other"] = rng.expr(width, 0);
        }
        if mode == "locals" {
            p["operator"] = json!(OPS[rng.next(OPS.len())]);
        }
        if mode == "loop" {
            p["operator"] = json!(["+", "-", "+%", "-%"][rng.next(4)]);
            p["count"] = json!(rng.next(7));
        }
        if mode == "short" {
            p["operator"] = json!(["&&", "||"][rng.next(2)]);
            p["left"] = json!(rng.next(2) == 0);
        }
        programs.push(p);
    }
    trace(programs, Some(seed))
}
pub fn fixed() -> Vec<(String, Value)> {
    let mut pins = Vec::new();
    for width in [32, 64] {
        let (min, max) = bounds(width);
        for e in [
            lit(min),
            lit(max),
            bin("+", lit(max), lit(1)),
            bin("-", lit(min), lit(1)),
            bin("*", lit(max), lit(2)),
            bin("/", lit(7), lit(0)),
            bin("/", lit(min), lit(-1)),
            bin("%", lit(min), lit(-1)),
            bin("+%", lit(max), lit(1)),
            bin("-%", lit(min), lit(1)),
            bin("*%", lit(max), lit(3)),
            bin("/", lit(-17), lit(5)),
            bin("%", lit(-17), lit(5)),
            json!({"kind":"neg","value":lit(min)}),
        ] {
            pins.push(program(width, "expr", e));
        }
        pins.push(program(width, "ct", bin("+", lit(20), lit(22))));
        pins.push(program(width, "ct", bin("+", lit(max), lit(1))));
        for op in ["&&", "||"] {
            for left in [false, true] {
                let mut p = program(width, "short", bin("/", lit(1), lit(0)));
                p["operator"] = json!(op);
                p["left"] = json!(left);
                pins.push(p);
            }
        }
        for count in [0, 3] {
            let mut p = program(width, "loop", lit(max));
            p["other"] = if count == 0 {
                bin("/", lit(1), lit(0))
            } else {
                lit(1)
            };
            p["operator"] = json!("+%");
            p["count"] = json!(count);
            pins.push(p);
        }
        let mut p = program(width, "branch", lit(1));
        p["other"] = bin("/", lit(1), lit(0));
        pins.push(p);
    }
    pins.chunks(8)
        .enumerate()
        .map(|(i, p)| (format!("scalar-{i}"), trace(p.to_vec(), None)))
        .collect()
}
pub fn mutation_case() -> Value {
    trace(
        vec![
            program(32, "expr", lit(2)),
            program(64, "expr", bin("-", bin("+", lit(8), lit(1)), lit(4))),
            program(32, "expr", lit(8)),
        ],
        Some(4),
    )
}
/// Typed AST candidates, bounded in count; step deletion is handled separately.
pub fn candidates(trace: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    let Some(actions) = trace["steps"].as_array() else {
        return out;
    };
    for (i, action) in actions.iter().enumerate() {
        let Ok(p) = Program::parse(&action["program"]) else {
            continue;
        };
        for (field, expr) in [("expr", Some(&p.expr)), ("other", p.other.as_ref())] {
            if let Some(expr) = expr {
                for candidate in expr.smaller() {
                    let mut t = trace.clone();
                    t["steps"][i]["program"][field] = candidate.json();
                    out.push(t);
                    if out.len() >= 128 {
                        return out;
                    }
                }
            }
        }
        if p.mode == "loop" && p.count > 0 {
            let mut t = trace.clone();
            t["steps"][i]["program"]["count"] = json!(p.count / 2);
            out.push(t);
        }
    }
    out
}
