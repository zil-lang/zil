resource Token { n: i32 }
fn f0(r: Token) { f1(r) }
fn f1(r: Token) { f2(r) }
fn f2(r: Token) { f3(r) }
fn f3(r: Token) { f4(r) }
fn f4(r: Token) { f5(r) }
fn f5(r: Token) { f6(r) }
fn f6(r: Token) { f7(r) }
fn f7(r: Token) { f8(r) }
fn f8(r: Token) { f9(r) }
fn f9(r: Token) { f10(r) }
fn f10(r: Token) { f11(r) }
fn f11(r: Token) { f12(r) }
fn f12(r: Token) { f13(r) }
fn f13(r: Token) { f14(r) }
fn f14(r: Token) { f15(r) }
fn f15(r: Token) { if r.n == 0 { r.n = 1; f0(r) } }
fn main() -> i32 { let r = Token { n: 0 }; f0(r); return r.n - 1 }
