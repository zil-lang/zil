struct Counter {
    value: i64
}
struct Token {
    address: &mut Counter
}
fn view(token: &Token) -> i64 {
    return token.address.value
}
fn check() -> i64 {
    var counter = Counter { value: 31 }
    let token = Token { address: &mut counter }
    let read = move () => view(&token)
    return read() + read()
}
