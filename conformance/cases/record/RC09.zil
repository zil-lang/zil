struct Point { x: i32; y: i32 }
fn mark(log: &mut i32, n: i32) -> i32 {
    *log = *log * 10 + n
    return n
}
fn base(log: &mut i32) -> Point {
    mark(log, 1)
    return Point { x: 0, y: 0 }
}
fn main() -> i32 {
    var log = 0
    let p = Point { ...base(&mut log), x: mark(&mut log, 2), y: mark(&mut log, 3) }
    return log
}
