fn check() -> i64 {
    let add: fn(i64) -> i64 = (value: i64) => value + 1
    return add(41)
}
