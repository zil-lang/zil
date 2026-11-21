import helpers

extern "C" fn putchar(value: i32) -> i32;

resource Token { mark: i32; value: i32 }
impl Drop for Token {
    fn drop(self: Self) { unsafe { putchar(.mark) } }
}
fn consume(give token: Token) -> i32 { return token.value }
fn make(mark: i32) -> impl FnOnce() -> i32 {
    let token = Token { mark: mark, value: 42 }
    return move () => {
        unsafe { putchar(67) }
        return consume(give token)
    }
}
fn main() -> i32 {
    let skipped = helpers.run_if(make(83), false)
    let called = helpers.run_if(make(68), true)
    if skipped == 0 && called == 42 { return 0 }
    return 1
}
