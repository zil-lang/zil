fn edit(s: &mut [i32]) -> i32 { s[0] += s[1]; return s[0] }
fn main() -> i32 { var values = [7, 9]; return edit(&mut values[..]) - 16 }
