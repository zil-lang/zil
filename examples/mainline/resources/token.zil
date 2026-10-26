import io

pub resource Token { code: i32 }

impl Drop for Token {
    fn drop(self: Self) { io.emit(.code) }
}

pub fn make(code: i32) -> Token contract { result(create) } {
    return Token { code }
}

pub fn relay(give token: Token) -> Token contract { result(give) } {
    return give token
}

pub fn bump(token: Token, amount: i32) {
    token.code = token.code + amount
}

pub fn read(token: Token) -> i32 { return token.code }

pub fn consume(give token: Token, amount: i32) -> i32 {
    bump(token, amount)
    return token.code
}
