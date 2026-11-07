import operations

pub struct Counter { n: i32 }

impl operations.Counter for Counter {
    fn get() -> i32 => .n
    fn set(n: i32) { .n = n }
    fn plus(other: &Self) -> i32 => .n + other.n
    fn make(n: i32) -> Self => Counter { n: n }
    fn finish() -> i32 => .n
}

pub fn make(n: i32) -> Counter => Counter.make(n)
