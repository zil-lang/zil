fn less(x: i64) -> bool { return 1 < x }
fn main() -> i32 { if less(4294967296) { return 42 } else { return 0 } }
