fn check() -> i64 {
    var value: i64 = 7
    let read = move () => value
    value = 9
    return read() * 100 + value
}
