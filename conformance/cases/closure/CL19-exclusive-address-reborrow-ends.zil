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
    let first = next()
    address.value += 3
    return first * 10 + address.value
}
