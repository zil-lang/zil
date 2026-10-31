import rules
pub struct Counter { n: i32 }
impl rules.Read for Counter { fn value() -> i32 => .n }
pub fn make(n: i32) -> Counter => Counter { n: n }
