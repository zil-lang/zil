import io
pub resource Token { code: i32 }
impl Drop for Token { fn drop(self: Self) { io.emit(.code) } }
pub fn token(code: i32) -> Token contract { result(create) } {
    return Token { code: code }
}
