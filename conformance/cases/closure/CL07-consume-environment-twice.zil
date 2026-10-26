struct Counter {
    value: i64
}
struct Token {
    address: &mut Counter
}
fn take(token: Token) -> i64 {
    return token.address.value
}
fn check() -> i64 {
    var counter = Counter { value: 31 }
    let token = Token { address: &mut counter }
    let consume = () => take(token)
    let first = consume()
    return first + consume() // expected-error: E-CLOSURE-ALREADY-CONSUMED
}
