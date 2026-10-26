extern "C" fn putchar(code:i32)->i32;
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code) } } }
fn main()->i32 {
    let token=Token{code:65};
    var next=move ()=>{let alias=token;alias.code+=1;token.code};
    if next()!=66 || next()!=67 {return 1}
    return 0
}
