import io

pub resource Token { pub code: i32 }
impl Drop for Token {
    fn drop(self: Self) { io.emit(.code) }
}
pub fn make(code: i32) -> Token contract { result(create) } {
    return Token { code }
}
pub fn read(token: Token) -> i32 { return token.code }
pub fn accept(give token: Token, amount: i32) -> i32 {
    io.emit(88)
    return token.code + amount
}
