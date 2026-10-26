fn check() -> i64 {
    var value: i64 = 7
    let read = () => value
    let before = read()
    value = 9
    return before * 100 + value
}
