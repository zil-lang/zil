fn check() -> i64 {
    var count: i64 = 0
    let next = () => {
        count += 1
        return count
    }
    return next() // expected-error: E-CALL-MUTABLE-BINDING
}
