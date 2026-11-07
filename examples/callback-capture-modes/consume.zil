extern "C" fn putchar(code:i32)->i32;
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code) } } }
fn consume(give token:Token) {}
fn main()->i32 {
    let first=Token{code:65};
    let second=Token{code:66};
    let once=move (take:bool)=>{
        if take {consume(give first)}
        second.code
    };
    if once(false)!=66 {return 1}
    let third=Token{code:67};
    let default_once=()=>consume(give third);
    default_once();
    return 0
}
