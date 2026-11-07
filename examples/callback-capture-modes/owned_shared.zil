extern "C" fn putchar(code:i32)->i32;
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code) } } }
fn main()->i32 {
    let token=Token{code:65};
    let read=move ()=>token.code;
    let moved=read;
    if moved()!=65 || moved()!=65 {return 1}
    return 0
}
