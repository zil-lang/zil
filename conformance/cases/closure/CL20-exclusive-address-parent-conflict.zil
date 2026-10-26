struct Counter {
    value: i64
}
fn check() -> i64 {
    var counter = Counter { value: 1 }
    let address = &mut counter
    var next = () => {
        address.value += 1
        return address.value
    }
    address.value += 3 // expected-error: E-CAPTURE-PARENT-WINDOW
    return next()
}
