fn edit(s: &[i32]) -> i32 { s[0] = 11; return s[0] }
fn main() -> i32 { let values = [7, 9]; return edit(&values[..]) }
