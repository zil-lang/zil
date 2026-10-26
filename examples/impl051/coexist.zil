extern "C" fn putchar(code:i32)->i32;
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code) } } }
fn make(code:i32)->Token contract { result(create) } { return Token{code} }
fn consume(give token:Token) {}
fn forward(give token:Token)->Token contract { result(give) } {
    let identity = (value:i32) => value;
    identity(0);
    return give token
}
fn main()->i32 {
    let token = make(65);
    let unrelated = (value:i64) => value + 1;
    let moved = forward(give token);
    if unrelated(40) == 41 { consume(give moved); return 0 }
    return 1
}
