import select

pub fn answer() -> i64 {
    var left: i64 = 40
    var right: i64 = 2
    let chosen = select.relay(&left, &right, 5)
    let snapshot: i64 = 3
    let read = move () => select.read(chosen) + snapshot
    let first = read()
    left = 100
    right = 200
    return first + left + right
}

fn main() -> i32 {
    if answer() == 305 { return 0 }
    return 1
}
