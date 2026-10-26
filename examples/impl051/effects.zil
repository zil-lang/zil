extern "C" fn putchar(code:i32)->i32;
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code) } } }
fn peek(token:Token)->i32 {return token.code}
fn bump(token:Token) {token.code+=1}
fn relay(token:Token,depth:i32) {
    let alias=token;
    if depth>0 {relay(alias,depth-1)} else {bump(alias)}
}
fn shared()->i32 {
    let token=Token{code:65};
    let read=move ()=>peek(token);
    if read()!=65 || read()!=65 {return 1}
    return 0
}
fn exclusive()->i32 {
    let token=Token{code:65};
    var update=move ()=>{relay(token,2);peek(token)};
    if update()!=66 || update()!=67 {return 1}
    return 0
}
fn main()->i32 {
    if shared()!=0 {return 1}
    if exclusive()!=0 {return 2}
    return 0
}
