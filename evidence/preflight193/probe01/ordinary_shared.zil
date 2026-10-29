fn read(s: &[i32]) -> i32 { return s[0] + s[1] }
fn main() -> i32 { let values = [7, 9]; return read(&values[..]) - 16 }
