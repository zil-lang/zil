fn main() -> i32 { var values = [7, 9]; let f = (s: &mut [i32]) => { s[0] += s[1]; return s[0] }; return f(&mut values[..]) - 16 }
