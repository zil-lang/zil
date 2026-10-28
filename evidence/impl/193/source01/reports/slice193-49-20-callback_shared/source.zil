fn main() -> i32 { let values = [7, 9]; let f = (s: &[i32]) => s[0] + s[1]; return f(&values[..]) - 16 }
