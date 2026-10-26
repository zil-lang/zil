// A shared address and a scalar snapshot live in the same inline environment.
pub fn answer() -> i64 {
    var source: i64 = 40
    let borrowed = &source
    var snapshot: i64 = 2
    let mixed = move () => *borrowed + snapshot
    let copied = mixed
    snapshot = 99
    let first = mixed() + copied()
    source = 100
    return first + source + snapshot
}

fn main() -> i32 {
    if answer() == 283 { return 0 }
    return 1
}
