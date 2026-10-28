fn main() -> i32 { let values = [7, 9]; let f = (s: &[i32]) => { s[0] = 11; return s[0] }; return f(&values[..]) }
