fn make() -> impl Fn(&[i32]) -> i32 { return (s: &[i32]) => s[0] }
fn main() -> i32 { let values = [7, 9]; let f = make(); return f(&values[..]) - 7 }
