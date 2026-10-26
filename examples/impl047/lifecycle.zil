extern "C" fn putchar(value: i32) -> i32;
resource Token { code: i32 }
impl Drop for Token {
    fn drop(self: Self) { unsafe { putchar(.code) } }
}
fn make(code: i32) -> Token contract { result(create) } {
    return Token { code }
}
fn read(token: Token) -> i32 { return token.code }
fn consume(give token: Token) {}
fn paths(which: i32) {
    let z = make(65)
    if which == 0 {
        let a = make(66)
        return
    }
    var n = 0
    while n < 3 {
        let item = make(67 + n)
        n += 1
        if n == 1 { continue }
        if n == 2 { break }
    }
    if true { let inner = make(70) }
}
fn maybe(flag: bool) {
    let token = make(74)
    if flag { consume(give token) }
}
fn main() -> i32 {
    paths(0)
    paths(1)
    read(make(71))
    make(72)
    maybe(true)
    maybe(false)
    for n in 0..2 { let token = make(75 + n) }
    return 0
}
