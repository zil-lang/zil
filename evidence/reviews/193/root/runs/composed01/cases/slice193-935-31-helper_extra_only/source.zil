fn apply<F: Fn() -> i32>(f: &F, s: &[i32]) -> i32 { return f() + s[0] }
fn main() -> i32 { let values = [7, 9]; let f = () => 9; return apply(&f, &values[..]) - 16 }
