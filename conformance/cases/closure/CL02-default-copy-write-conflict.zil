fn check() -> i64 {
    var value: i64 = 7
    let read = () => value
    value = 9 // expected-error: E-CAPTURE-LOAN-CONFLICT
    return read()
}
