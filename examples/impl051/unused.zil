extern "C" fn putchar(code:i32)->i32;
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code) } } }
fn consume(give token:Token) {}
fn main()->i32 {
    if true {
        let first=Token{code:65};
        let second=Token{code:66};
        let unused=move ()=>first.code+second.code;
    }
    if true {
        let third=Token{code:67};
        let never_called=()=>consume(give third);
    }
    return 0
}
