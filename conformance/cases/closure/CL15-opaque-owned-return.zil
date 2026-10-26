fn make(value: i64) -> impl Fn() -> i64 {
    return move () => value
}
fn check() -> i64 {
    let read = make(41)
    return read() + 1
}
