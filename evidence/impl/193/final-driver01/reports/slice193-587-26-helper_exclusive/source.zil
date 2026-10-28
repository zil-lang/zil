fn apply<F: Fn(&mut [i32]) -> i32>(f: &F, s: &mut [i32]) -> i32 { return f(s) }
fn main() -> i32 { var values = [7, 9]; let f = (s: &mut [i32]) => { s[0] += s[1]; return s[0] }; return apply(&f, &mut values[..]) - 16 }
