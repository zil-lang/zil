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
    let before = token.address.value // expected-error: E-USE-AFTER-MOVE
    return consume() + before
}
