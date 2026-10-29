fn apply<F: Fn(&[i32]) -> i32>(f: &F, s: &[i32]) -> i32 { return f(s) }
fn main() -> i32 { let values = [7, 9]; let f = (s: &[i32]) => s[0] + s[1]; return apply(&f, &values[..]) - 16 }
