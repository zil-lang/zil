extern "C" fn putchar(code:i32)->i32;
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code) } } }
fn early() {
    let first=Token{code:65};
    let read_first=move ()=>first.code;
    let second=Token{code:66};
    let read_second=move ()=>second.code;
    return
}
fn main()->i32 {
    early();
    var n=0;
    while n<3 {
        let token=Token{code:67+n};
        let unused=move ()=>token.code;
        n+=1;
        if n==1 {continue}
        if n==2 {break}
    }
    let immediate=Token{code:69};
    if (move ()=>immediate.code)()!=69 {return 1}
    let discarded=Token{code:70};
    move ()=>discarded.code;
    return 0
}
