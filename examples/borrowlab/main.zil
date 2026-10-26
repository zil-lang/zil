// Native example: default shared capture, explicit move snapshot, last-use end.
pub fn answer() -> i64 {
    var base: i64 = 40
    let snapshot = move () => base
    let borrowed = x => base + x
    let copied = borrowed
    let first = borrowed(1) + copied(2)
    base = 100
    return first + snapshot() + base
}

fn main() -> i32 {
    if answer() == 223 { return 0 }
    return 1
}
