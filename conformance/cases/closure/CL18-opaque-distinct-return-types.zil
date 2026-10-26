fn make(first: bool) -> impl Fn() -> i64 {
    if first {
        let left: i64 = 1
        return move () => left
    } else {
        let right: i64 = 2
        return move () => right // expected-error: E-OPAQUE-RETURN-TYPE
    }
}
