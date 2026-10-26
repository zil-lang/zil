fn check() -> i64 {
    let twice = move (value: i64) -> i64 => { value * 2 }
    return twice(21)
}
