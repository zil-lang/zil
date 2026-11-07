extern "C" fn putchar(code:i32)->i32;
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code) } } }
fn main()->i32 {
    let token=Token{code:65};
    let read=()=>token.code;
    if read()!=65 {return 1}
    token.code=66;
    if read()!=66 {return 2}
    let alias=token;
    let write=()=>{alias.code=67};
    write();
    if read()!=67 {return 3}
    return 0
}
