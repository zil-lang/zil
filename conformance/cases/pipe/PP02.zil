fn identity(x: i32) -> i32 => x
fn source(log: &mut i32) -> i32 {
    *log = *log * 10 + 1
    return 7
}
fn target(log: &mut i32) -> fn(i32) -> i32 {
    *log = *log * 10 + 2
    return identity
}
fn main() -> i32 {
    var log = 0
    let value = source(&mut log) |> target(&mut log)
    return log
}
