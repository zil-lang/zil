fn make() -> impl Fn() -> i64 {
    let value: i64 = 41
    return () => value // expected-error: E-CAPTURE-ESCAPE
}
