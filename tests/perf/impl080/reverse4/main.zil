resource Token { n: i32 }
fn f0(r: Token) { f1(r) }
fn f1(r: Token) { f2(r) }
fn f2(r: Token) { f3(r) }
fn f3(r: Token) { r.n = 1 }
fn main() -> i32 { let r = Token { n: 0 }; f0(r); return r.n - 1 }
