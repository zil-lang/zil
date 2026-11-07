extern "C" fn putchar(value: i32) -> i32;
resource Token { code: i32 }
impl Drop for Token {
    fn drop(self: Self) {
        unsafe { putchar(.code) }
    }
}
fn make(code: i32) -> Token contract { result(create) } {
    return Token { code }
}
fn take(give token: Token) -> i32 {
    return token.code
}
fn forward(give token: Token) -> Token contract { result(give) } {
    return give token
}
fn read(token: Token) -> i32 { return token.code }
fn main() -> i32 {
    let z = make(65)
    let alias = z
    alias.code = 66
    let a = make(67)
    read(alias)
    take(give a)
    let moved = forward(give z)
    return read(moved) - 66
}
