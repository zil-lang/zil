fn hidden() -> i32 => 8
pub fn value(extra: i32) -> i32 => hidden() + extra
fn main() -> i32 => value(0) - 7
