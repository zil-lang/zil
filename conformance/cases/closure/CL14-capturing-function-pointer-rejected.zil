fn check() -> i64 {
    let offset: i64 = 1
    let add: fn(i64) -> i64 = (value: i64) => value + offset // expected-error: E-CAPTURED-FN-POINTER
    return add(41)
}
