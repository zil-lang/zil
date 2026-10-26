struct Counter {
    value: i64
}
fn make() -> impl Fn() -> i64 {
    let counter = Counter { value: 41 }
    let address = &counter
    return move () => address.value // expected-error: E-CAPTURE-ESCAPE
}
